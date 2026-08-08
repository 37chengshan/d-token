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

当前能力对照基线（2026-08-08 三候选审计后）：

- 上游：`headroomlabs-ai/headroom`（官方 remote：`https://github.com/headroomlabs-ai/headroom.git`；本地只读研究副本：`/Users/cc/Documents/d-token-external/headroom/headroom-main-2026-08-08`，已核验三候选 commit 均在仓内）
- **当前比较 pin（唯一）**：`v0.34.0` / `9fd5ae3d530b5d9eb9e9127e413235a5a3b2faf6`（2026-08-05 发布；`pyproject.toml` `version = "0.34.0"`；2026-08-08 完成三候选漂移审计后定为唯一 current comparison pin，替代旧 main-HEAD 基线）
- 历史 pin：`01df2452529a86c689cf226fecd5918cc5d19676`（2026-08-02 获取的 main 分支 HEAD，`pyproject.toml` `version = "0.33.0"`；被 v0.34.0 取代，只用于解释旧迁移证据，不再作为完成口径）
- 前一发布基线（历史）：`v0.33.0` / `28aa53dc7ca51e687cc719c3fe160f3be50c6570`（2026-07-29 发布）
- 历史参考 commit：`1725cd1f8376828849c0e48fd9008afa25d24fff`；只用于解释 2026-07 的旧证据
- **main post-release delta（跟踪，不 pin）**：`7f6950be34e29304deae0fa5138b852491b092fe`（2026-08-08 获取；`pyproject.toml` `version` 仍为 `0.34.0`，未发布）
- 许可证：Apache-2.0（三个候选 pin 的根 `LICENSE` 均为 Apache-2.0 原文，无差异）
- NOTICE：上游仓库同时包含 `NOTICE`；v0.34.0 起新增 "Vendored dashboard assets" 段（`headroom/dashboard/static/`：Tailwind CSS 3.4.17 MIT、htmx 1.9.10 Zero-Clause BSD、Alpine.js 3.13.3 MIT，由 #2734 将 dashboard 静态资源本地化引入）；v0.34.0 → main 无 NOTICE 变化
- 当前方式：行为与接口对照，现有 d-token Rust 实现未直接复制上游源码；d-token 不复制、不启动、不依赖外部 Headroom 8787 服务（无 Python sidecar、无进程内插件、无远端 Dashboard 数据源）；生产能力必须由 d-token-owned Rust 实现证明
- v0.33 首批吸收边界：Rust-owned compressor registry 与显式 route-bound `/v1/compress` gateway surface；不加载 Python entry point 或第三方进程内插件
- v0.33.0 → 旧基线 01df2452 迁移决策（2026-08-03，当时 main HEAD）：共 14 commit / 151 文件 / +5568 −11786。已吸收：PHP codec（`CodeLanguage::Php`、tree-sitter-php@0.23.11、gate evidence 与 parity 测试 9/9 通过）。核查为不适用：CCR bare-marker 守卫（#2694）—— d-token 全仓不生成 `<<ccr:` 内联 marker，该缺陷类在设计上不存在，无需移植。契约对齐评估（#2718/#2691，2026-08-03 完成）：(a) marker-free 默认（#2691）已由构造满足——d-token 任何 codec 都不发射 `<<ccr:` 内联 marker，无需改动；(b) `mode` 校验（#2691）不吸收——`COMPRESS_MODES=(ccr, lossy_inline, lossless_then_lossy)` 中的 lossy/Kompress 管线 d-token 不具备，`CompressionDecisionRequest` 的 `deny_unknown_fields` 已对未知 mode fail-closed 返回 400；(c) frozen_message_count（#2718）评估为有价值缺口——`context_pipeline::compute_frozen_message_count` 已按 `cache_control` marker 实现同源 floor（注释自述 #2718 parity），但缺少请求级 `config.frozen_message_count` 覆盖入口，agent-loop 调用方无法固定缓存前缀；吸收需跨 `CompressionDecisionRequest`/HTTP/MCP/TS SDK 四面契约与 parity 测试，列为后续独立任务。记录上游移除：RTK/lean-ctx（#2677）。不在范围：Kompress ML、Codex Live voice、Serena wrap。
- 完成口径：以 `docs/PRD.md` §8 和 `docs/PLAN.md` 的逐能力证据矩阵为准；外部 Headroom、fixture、metadata-only 端点或同名接口不得算已迁移

## 三候选漂移审计与唯一 pin 决议（2026-08-08）

候选：旧基线 `01df2452529a86c689cf226fecd5918cc5d19676`、稳定 release `v0.34.0` / `9fd5ae3d530b5d9eb9e9127e413235a5a3b2faf6`、main `7f6950be34e29304deae0fa5138b852491b092fe`。

- **决议：唯一 current comparison pin = `v0.34.0` / `9fd5ae3d`**。保守默认优先稳定 release：v0.34.0 是首个包含已文档化 `/v1/compress` 契约（#2743：按 model 解析 tokenizer、路由不做格式转换）的发布，也是 d-token Rust-owned gateway surface 对齐的版本；`01df2452` 降为历史 pin（它只是 2026-08-02 抓取时的 main HEAD，先于 v0.34.0，且缺 v0.34.0 的 CVE 依赖修复 #2753 与 NOTICE 静态资源登记）；main `7f6950be` 降为 post-release delta（跟踪不 pin）。
- **`01df2452` → `v0.34.0`（33 commit / 129 文件 / +8936 −1021）**：许可证无变化（Apache-2.0）；NOTICE 新增 vendored dashboard 静态资源段（#2734）；接口面变化——`/v1/compress` 契约文档化（#2743）、qualified CCR 名称（#2698）、ContentRouter 不再对 `headroom_retrieve` 结果二次压缩（#2654）、Anthropic tools 数组不再 toggle `headroom_retrieve`（#2672）、Kompress 运行期拒绝坏 artifact 与启动预取（#2740）、Kompress 自托管推理栈（#2736）、匿名压缩统计（#2728）、aiohttp/cryptography CVE 依赖升级（#2753）。无新 codec、无 compressor registry 结构性变化。均与 d-token 对照面兼容；无必须吸收项。
- **`v0.34.0` → main `7f6950be`（33 commit / 74 文件 / +4874 −428）**：许可证与 NOTICE 无变化；无 codec、无 `/v1/compress` 契约、无 compressor registry 变化。内容集中在 proxy 行为修复（tool-search deferral #2539/#2696/#2729、Codex WS 压缩帧 #2823、Hermes tool_call bridge #2717、CCR marker 注入策略 #2848）、CCR 内部 TTL 边界（#2669 + 测试 #2794）、telemetry/pricing/CLI 扩展缝（#2802）、per-request backend routing（#2809）、热路径性能 −27%（#2838）、上游自身依赖 CVE 升级（h2 #2839）与安装器/容器修复。**无必须采用 main 的实际证据**：d-token 不消费上游代码（行为/接口对照 + Rust-owned 实现），上游修复不构成采纳义务；其中 CCR marker 注入策略（#2848）与 SQLite TTL 边界（#2669）作为 parity 跟踪项记录——d-token Rust CCR 已独立实现加密/TTL/retrieve/delete，无需移植。
- **跟踪但不 pin**：main 上持续出现的 proxy 行为修复与新增接口（CLI extension seam #2802、route_advice #2809）留作下次 release 审计输入；d-token 不对未发布 main 承诺任何对照基线。

## LeanCTX / Context Bus / Context Relay 来源登记（2026-08-06）

依据 `docs/PRD.md` P0.0.5，LeanCTX 与 CCR 是独立于 Headroom 的能力域，必须分别登记来源、许可证、参考版本、变更范围、能力矩阵和发布证据。

- 上游项目：`yvgude/lean-ctx`（独立开源项目；headroom 上游 #2677 已于 2026-07-31 合并移除对 rtk/lean-ctx 的下载、安装与配置，二者不再有集成关系）
- 当前固定 release：`v3.9.17`（2026-08-04 发布）
- 固定 commit：`cf8fce6bb1759a845088988cca189beaf70b05a9`
- 许可证：Apache-2.0（项目总许可；根 `LICENSE`、README、Rust `Cargo.toml` 与 Claude plugin `manifest.json` 均声明 Apache-2.0）
- 子包例外（声明 MIT，单独列出，不并入项目总许可）：`clients/python`（`pyproject.toml`）、`rust/crates/grammar-addons/lua`（`Cargo.toml`）
- NOTICE：仓库包含 `NOTICE`（`lean-ctx`，Copyright 2026 Yves Gugger，含第三方贡献者列表）
- 本地只读研究副本：`/Users/cc/Documents/d-token-external/lean-ctx/`（git clone，HEAD 固定 v3.9.17 / `cf8fce6b`；另有官方 source archive 与 SHA-256 `a1178c68…deac25` 已核验）
- 当前方式：仅行为与接口对照；d-token 不复制源码、不引入运行依赖、不把研究记为迁移完成
- d-token 差异/边界：PRD P0.0.5 的九条能力边界（Context Bus、Session Cache、Context Relay、Query-aware Retrieval、Recovery Pointer、Stale/Superseded、SharedContext、同 Agent 自命中防护与收益分离计量）必须由 d-token-owned Rust service 提供；无真实跨 Agent 证据时只能 `partial`/`unavailable`，不得因 Headroom codec 或 CCR 通过而自动变绿

## CCR 来源判定（2026-08-06）

- 判定：CCR **不是独立开源项目**。按 headroom 官方源码与文档（README "Reversible (CCR)"、`headroom_retrieve` MCP 工具、`<<ccr:` 内联 marker），CCR 是 headroom 仓库内的可逆压缩存储能力（CacheAligner → ContentRouter → CCR 管线的一部分），没有独立上游仓库。
- 来源：随 headroom 基线登记——`headroomlabs-ai/headroom` 当前比较 pin `v0.34.0` / `9fd5ae3d530b5d9eb9e9127e413235a5a3b2faf6`（Apache-2.0 + NOTICE），无需单独克隆。
- d-token 差异：d-token 的 CCR 为 Rust-owned 实现（加密、TTL、配额、权限、retrieve/delete、崩溃恢复、精确删除），与 LeanCTX 职责分离（PRD P0.0.5 表格）；普通日志/Dashboard/Ledger/export 只保存 metadata，不保存 CCR 原文。
