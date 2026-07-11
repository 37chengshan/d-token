# d-token SDK（预留）

更新时间：2026-07-11 晚间

本目录是未来 TypeScript SDK/shared types 的预留位置，当前没有可发布 SDK、稳定 API 版本或 npm package。

在正式实现前，不得在 README、PRD、UI 或发布材料中把该目录描述为已交付的外部集成能力。当前共享数据合同以 Rust 类型、Tauri command 和 `dtoken-core-api` 实际响应为准。

未来实现需要先明确：

1. 稳定版本和兼容策略。
2. Provider/Route/Instance/Event 的公开字段与脱敏边界。
3. 本地 loopback API 的认证与权限模型。
4. 与桌面 App 的单一数据合同，避免复制类型后漂移。
