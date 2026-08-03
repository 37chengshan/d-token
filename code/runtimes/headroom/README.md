# Headroom-derived runtime boundary

更新时间：2026-08-02

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
- 当前参考 commit：`01df2452529a86c689cf226fecd5918cc5d19676`（main 分支 HEAD，2026-08-02 获取，尚未发布 tag）
- 前一发布基线：`v0.33.0` / `28aa53dc7ca51e687cc719c3fe160f3be50c6570`（2026-07-29 发布）
- 历史参考 commit：`1725cd1f8376828849c0e48fd9008afa25d24fff`；只用于解释旧迁移证据，不再作为完成口径
- 许可证：Apache-2.0；上游仓库同时包含 `NOTICE`
- 当前方式：行为与接口对照，现有 d-token Rust 实现未直接复制上游源码
- v0.33 首批吸收边界：Rust-owned compressor registry 与显式 route-bound `/v1/compress` gateway surface；不加载 Python entry point 或第三方进程内插件
- v0.33.0 → main HEAD 迁移决策（2026-08-03）：共 14 commit / 151 文件 / +5568 −11786。已吸收：PHP codec（`CodeLanguage::Php`、tree-sitter-php@0.23.11、gate evidence 与 parity 测试 9/9 通过）。核查为不适用：CCR bare-marker 守卫（#2694）—— d-token 全仓不生成 `<<ccr:` 内联 marker，该缺陷类在设计上不存在，无需移植。记录为契约对齐候选（未吸收）：`/v1/compress` frozen_message_count（#2718）与 marker-free/mode 校验（#2691）—— d-token 请求面 `deny_unknown_fields` fail-closed 且默认 marker-free，需跨 HTTP/MCP/SDK parity 另行评估。记录上游移除：RTK/lean-ctx（#2677）。不在范围：Kompress ML、Codex Live voice、Serena wrap。
- 完成口径：以 `docs/PRD.md` §8 和 `docs/PLAN.md` 的逐能力证据矩阵为准；外部 Headroom、fixture、metadata-only 端点或同名接口不得算已迁移
