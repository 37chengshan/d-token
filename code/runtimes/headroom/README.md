# Headroom-derived runtime boundary

更新时间：2026-08-08

本目录记录 d-token 对 Headroom-derived 能力的来源、兼容、迁移和审计边界。

它不是：

- Python sidecar 入口。
- 外部 Headroom 8787 服务的启动目录。
- Dashboard 的远端数据源。
- 独立可发布 runtime。

生产能力必须由 `code/crates/` 中的 d-token-owned Rust 实现、`dtoken-core-api`、Event Ledger 和当前测试证据证明。

后续引入或迁移上游实现时，必须记录许可证、NOTICE、来源 commit、修改范围和仍未迁移的行为。

当前能力对照基线：

- 上游：`headroomlabs-ai/headroom`
- 当前唯一参考：`v0.34.0` / `9fd5ae3d530b5d9eb9e9127e413235a5a3b2faf6`（2026-08-05 发布；已完成相对旧基线 `01df2452` 与后续 main `7f6950be` 的增量、许可证和 NOTICE 对比）
- 旧基线 `01df2452529a86c689cf226fecd5918cc5d19676` 仅保留为历史复现；main `7f6950be` 只作发布后差异跟踪，不构成第二 pin
- 前一发布基线：`v0.33.0` / `28aa53dc7ca51e687cc719c3fe160f3be50c6570`（2026-07-29 发布）
- 历史参考 commit：`1725cd1f8376828849c0e48fd9008afa25d24fff`；只用于解释旧迁移证据，不再作为完成口径
- 许可证：Apache-2.0；上游仓库同时包含 `NOTICE`
- 当前方式：行为与接口对照，现有 d-token Rust 实现未直接复制上游源码
- v0.33 首批吸收边界：Rust-owned compressor registry 与显式 route-bound `/v1/compress` gateway surface；不加载 Python entry point 或第三方进程内插件
- v0.33.0 → main HEAD 迁移决策（2026-08-03）：共 14 commit / 151 文件 / +5568 −11786。已吸收：PHP codec（`CodeLanguage::Php`、tree-sitter-php@0.23.11、gate evidence 与 parity 测试 9/9 通过）。核查为不适用：CCR bare-marker 守卫（#2694）—— d-token 全仓不生成 `<<ccr:` 内联 marker，该缺陷类在设计上不存在，无需移植。契约对齐评估（#2718/#2691，2026-08-03 完成）：(a) marker-free 默认（#2691）已由构造满足——d-token 任何 codec 都不发射 `<<ccr:` 内联 marker，无需改动；(b) `mode` 校验（#2691）不吸收——`COMPRESS_MODES=(ccr, lossy_inline, lossless_then_lossy)` 中的 lossy/Kompress 管线 d-token 不具备，`CompressionDecisionRequest` 的 `deny_unknown_fields` 已对未知 mode fail-closed 返回 400；(c) frozen_message_count（#2718）评估为有价值缺口——`context_pipeline::compute_frozen_message_count` 已按 `cache_control` marker 实现同源 floor（注释自述 #2718 parity），但缺少请求级 `config.frozen_message_count` 覆盖入口，agent-loop 调用方无法固定缓存前缀；吸收需跨 `CompressionDecisionRequest`/HTTP/MCP/TS SDK 四面契约与 parity 测试，列为后续独立任务。记录上游移除：RTK/lean-ctx（#2677）。不在范围：Kompress ML、Codex Live voice、Serena wrap。
- 完成口径：以 `docs/PRD.md` §8 和 `docs/PLAN.md` 的逐能力证据矩阵为准；外部 Headroom、fixture、metadata-only 端点或同名接口不得算已迁移

## LeanCTX / Context Bus / Context Relay 来源登记（2026-08-06）

依据 `docs/PRD.md` P0.0.5，LeanCTX 与 CCR 是独立于 Headroom 的能力域，必须分别登记来源、许可证、参考版本、变更范围、能力矩阵和发布证据。

- 上游项目：`yvgude/lean-ctx`（独立开源项目；headroom 上游 #2677 已于 2026-07-31 合并移除对 rtk/lean-ctx 的下载、安装与配置，二者不再有集成关系）
- 当前固定 release：`v3.9.17`（2026-08-04 发布）
- 固定 commit：`cf8fce6bb1759a845088988cca189beaf70b05a9`
- 许可证：Apache-2.0（项目总许可；根 `LICENSE`、README、Rust `Cargo.toml` 与 Claude plugin `manifest.json` 均声明 Apache-2.0）
- 子包例外（声明 MIT，单独列出，不并入项目总许可）：`clients/python`（`pyproject.toml`）、`rust/crates/grammar-addons/lua`（`Cargo.toml`）
- NOTICE：仓库包含 `NOTICE`（`lean-ctx`，Copyright 2026 Yves Gugger，含第三方贡献者列表）
- 本地只读研究副本：`/Users/cc/Documents/d-token-external/lean-ctx/`（git clone，HEAD 固定 v3.9.17 / `cf8fce6b`；另有官方 source archive 与 SHA-256 `a1178c68…deac25` 已核验）
- 唯一 pin 决议（2026-08-08，PR-L0）：**`v3.9.17` / `cf8fce6bb1759a845088988cca189beaf70b05a9` 为唯一参考 pin**；上游 main `95a1b7bc60a01479986af026c0c24451804993ba`（2026-08-08 克隆 `lean-ctx-main-2026-08-08`）评估后仅作 post-pin 研究，不作为第二 pin，漂移 D4 由此闭合
- main delta 评估（`cf8fce6b..95a1b7bc`，93 commit / 160 文件 / +11209 −2665，线性历史，`v3.9.17` 为祖先）：
  - 版本事实：workspace `rust/Cargo.toml` 与 Claude plugin `manifest.json` 版本号均未变（3.9.17 / 3.3.6），93 个提交全部未发布；`v3.9.17`（2026-08-04 tag）是唯一稳定 release tag
  - 主要内容：ADR-001..013 与 System Inventory/Architecture Freeze；新增 `rust/crates/lean-ctx-protocol` v0.1.0 crate（wire contracts、SavingsObservationV1、policy 类带过期语义）；enterprise 特性门控（`lean-ctx enterprise init`、`[enterprise]` 配置与 `x-leanctx-*` header 注入、proxy/policy_gate/gateway_identity 与 dashboard/usage_breakdown 仅 enterprise 编译）；r15–r18 "science-driven context intelligence" 系列（agent-01..14：ib/cognitive/fsrs/anti-interrupt/wasserstein/graph-expand/mdl-mode/verbosity/prefetch/stigmergy/integration/cli/mcp/parity，后续仍带 clippy/断言修复提交 → 进行中 WIP）；上游自身 OSS/专有边界调整（ADR-010 "OSS and Proprietary Contribution Boundary"、`security: remove proprietary docs` 78219bc9）
  - 安全修复（v3.9.17 不含，仅作 d-token 自有 Rust 实现对照情报）：#1428 shadow mode 下 honor IDE 权限规则、#1431/#1432 dot-dir glob 遍历与 firewall 指引、#1433/#1434/#1435 patch 原子性/MethodNotFound/rules-MCP gate、allowlist 委托包装检查（e5266b19）、#1418 MCP `file://` URI、#1437 sticky reroot（e75afa58）
  - 许可证在 delta 内零变化（已逐项核对）：根 `LICENSE`/`NOTICE`、README、workspace `Cargo.toml`（`license = "Apache-2.0"`）、`.claude-plugin/manifest.json`（`"license": "Apache-2.0"`）均不变；子包 MIT 例外（`clients/python` `pyproject.toml`、`rust/crates/grammar-addons/lua` `Cargo.toml`）不变；delta 新增 `lean-ctx-protocol` 声明 Apache-2.0
- 决议理由：d-token 对 lean-ctx 仅做公开行为/接口对照（不复制源码、不引入运行依赖），稳定 tag 是唯一可引用参考事实；main 为未发布开发线（WIP 系列 + enterprise 门控 + 版本号未升），不满足 pin 条件。上游发布新 release tag 后再评估是否更新 pin；任何 main 能力进入 d-token 评估都必须引用本 delta 记录而非 main HEAD 本身
- 当前方式：仅行为与接口对照；d-token 不复制源码、不引入运行依赖、不把研究记为迁移完成
- d-token 差异/边界：PRD P0.0.5 的九条能力边界（Context Bus、Session Cache、Context Relay、Query-aware Retrieval、Recovery Pointer、Stale/Superseded、SharedContext、同 Agent 自命中防护与收益分离计量）必须由 d-token-owned Rust service 提供；无真实跨 Agent 证据时只能 `partial`/`unavailable`，不得因 Headroom codec 或 CCR 通过而自动变绿

## CCR 来源判定（2026-08-06）

- 判定：CCR **不是独立开源项目**。按 headroom 官方源码与文档（README "Reversible (CCR)"、`headroom_retrieve` MCP 工具、`<<ccr:` 内联 marker），CCR 是 headroom 仓库内的可逆压缩存储能力（CacheAligner → ContentRouter → CCR 管线的一部分），没有独立上游仓库。
- 来源：随 headroom 基线登记——`headroomlabs-ai/headroom` 参考 commit `01df2452529a86c689cf226fecd5918cc5d19676`（Apache-2.0 + NOTICE），无需单独克隆。
- d-token 差异：d-token 的 CCR 为 Rust-owned 实现（加密、TTL、配额、权限、retrieve/delete、崩溃恢复、精确删除），与 LeanCTX 职责分离（PRD P0.0.5 表格）；普通日志/Dashboard/Ledger/export 只保存 metadata，不保存 CCR 原文。

## RTK（integrated/managed 来源登记）（2026-08-08）

依据 `docs/PRD.md` §0.3/§4.2/§7，RTK 不作为独立产品路径，只作为 runtime 内部能力登记来源：

- 上游项目：`rtk-ai/rtk`；官方 remote：`https://github.com/rtk-ai/rtk.git`
- 唯一参考 pin：`v0.45.0` / `b34be37caf3796b69a50952a28e60e32b5daad43`（2026-08-07 发布；git tag `v0.45.0` 与 `latest` 均指向该 commit；`Cargo.toml` `version = "0.45.0"`）
- 许可证：Apache-2.0（根 `LICENSE`，2026-08-08 核验，SHA-256 `4044ade9…87cfd`；文末以 `Copyright 2024 rtk-ai and rtk-ai Labs` 版权行替换附录样板）
- NOTICE：固定 commit 无根目录 `NOTICE`（全仓无 `NOTICE*` 文件）
- 本地只读研究副本：`/Users/cc/Documents/d-token-external/rtk-v0.45.0/`（git clone，HEAD 固定 v0.45.0 / `b34be37`）
- 边界：integrated/managed RTK 是 **d-token-owned 行为迁移边界，不是隐式 external runtime**——`dtoken-preprocessor` 的 managed RTK lifecycle（command rewrite、shim/hook、fail-safe、usage metadata、精确卸载）为 d-token 自有实现；d-token 不独立分发 RTK binary、不默认安装 standalone RTK、不把外部 RTK 当运行依赖；外部 standalone RTK 只读检测仅作冲突提示（PRD §13.2/§7.8）
- `rtk gain`：RTK 本地 savings 汇总命令，不作为 d-token 产品轮询路径；RTK savings 只来自 d-token runtime metrics 或 integrated RTK metrics（PRD §10/§15）
- 当前方式：行为与接口对照；d-token 未复制该 commit 源码、无运行依赖
- 后续边界：若迁移或修改上游源码，必须在代码进入仓库前补充许可证副本、NOTICE、逐文件来源、修改范围和保留版权声明（PRD §18/§21）
