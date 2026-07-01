# code

本目录存放 d-token 的全部代码。

规划：

```text
code/apps/desktop                Tauri 2 + React/TypeScript 桌面 App
code/crates/dtoken-proxy         本地 proxy
code/crates/dtoken-router        provider 与 route decision
code/crates/dtoken-compress      compression interface
code/crates/dtoken-core-api      Desktop 调用的 local API
code/crates/dtoken-process       本地进程管理
code/packages/sdk                TypeScript SDK / shared types
code/sidecars/headroom-compatible  Headroom-compatible Python runtime
code/configs                     示例配置，不放真实密钥
```

所有正式文档放 `docs/`，所有测试、脚本和杂项放 `support/`。
