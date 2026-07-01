# code

本目录存放 d-token 的全部代码。

当前 M1 结构：

```text
code/Cargo.toml                         Rust workspace
code/apps/desktop                       Tauri 2 + React/TypeScript 桌面 App 占位
code/crates/dtoken-types                共享领域模型
code/crates/dtoken-config               本地配置加载
code/crates/dtoken-router               provider 与 route decision
code/crates/dtoken-compress             compression interface
code/crates/dtoken-core-api             Desktop 调用的 local API
code/crates/dtoken-process              本地进程管理
code/crates/dtoken-doctor               doctor 检查
code/crates/dtoken-proxy                本地 proxy / core-api 启动入口
code/packages/sdk                       TypeScript SDK / shared types 占位
code/runtimes/headroom                  Headroom-compatible runtime 占位
code/configs/config.example.json        安全示例配置，不放真实密钥
```

所有正式文档放 `docs/`，所有测试、脚本和杂项放 `support/`。
