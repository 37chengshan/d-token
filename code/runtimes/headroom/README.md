# Headroom-derived runtime boundary

更新时间：2026-07-13

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
- 参考 commit：`1725cd1f8376828849c0e48fd9008afa25d24fff`（2026-07-13 获取）
- 许可证：Apache-2.0；上游仓库同时包含 `NOTICE`
- 当前方式：行为与接口对照，现有 d-token Rust 实现未直接复制该 commit 的源码
- 完成口径：以 `docs/PRD.md` §8 和 `docs/PLAN.md` 的逐能力证据矩阵为准；外部 Headroom、fixture、metadata-only 端点或同名接口不得算已迁移
