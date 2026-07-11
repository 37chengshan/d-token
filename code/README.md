# d-token 代码目录

更新时间：2026-07-11 晚间

`code/` 包含 d-token 的正式产品代码。当前主线是 Rust-owned runtime + Tauri 2 + React/TypeScript，不包含 Python sidecar 产品运行时。

## 当前结构

```text
code/
├── Cargo.toml                  Rust workspace
├── apps/desktop/               唯一正式桌面 App
├── crates/                     13 个 Rust 业务 crate
├── configs/                    安全示例配置
├── runtimes/headroom/          Headroom-derived provenance/迁移边界
├── packages/sdk/               尚未实现的 SDK 预留目录
└── UI design for project/      设计原型/历史 UI 输入，不是生产入口
```

## Rust workspace

| Crate | 当前职责 |
|---|---|
| `dtoken-types` | Provider、Route、Instance、Request、Event 等共享领域模型 |
| `dtoken-config` | active config、Provider/Route/Instance 与 Optical 配置加载/校验 |
| `dtoken-router` | 显式路由选择、协议/模型匹配和无静默 fallback 决策 |
| `dtoken-compress` | 压缩策略、Headroom-derived pipeline、Optical shadow planner |
| `dtoken-process` | 本地进程、端口和实例生命周期基础 |
| `dtoken-doctor` | Doctor 检查模型与诊断结果 |
| `dtoken-event-ledger` | 脱敏事件、JSONL 持久化和查询基础 |
| `dtoken-preprocessor` | integrated RTK/preprocessor 能力边界 |
| `dtoken-headroom-adapter` | Headroom-derived 能力兼容与迁移适配 |
| `dtoken-project-policy` | 项目压缩策略和规则推荐 |
| `dtoken-config-patcher` | 配置计划、diff、backup 和 rollback 基础 |
| `dtoken-core-api` | loopback local API、协议入口、Dashboard/Doctor/Events/Export |
| `dtoken-proxy` | d-token Rust runtime 启动入口和本地 listener |
| `d-token-desktop` | Tauri native commands、凭据、服务生命周期和平台打包 |

## 桌面 App

`apps/desktop/` 是唯一正式 UI 和桌面运行入口：

```text
apps/desktop/src/app/            React 18 产品 UI
apps/desktop/src-tauri/          Tauri 2/Rust native 层
apps/desktop/src-old/            旧前端实现，不得新增功能
```

正式一级页面为：总体概览、安装与连接、分析、Doctor、变更与回退、设置。

## 当前运行链路

```text
AI coding agent
→ 127.0.0.1 d-token listener
→ dtoken-core-api / dtoken-proxy
→ compression + explicit route decision
→ configured upstream
→ redacted Event Ledger / Dashboard / Doctor / App
```

## 重要边界

1. `runtimes/headroom/` 不是 Python runtime，也不启动外部 Headroom dashboard。
2. `packages/sdk/` 只是预留目录，不能写成已发布 SDK。
3. `UI design for project/` 和 `apps/desktop/src-old/` 不是生产代码来源。
4. Optical 当前只有 shadow planning 与 metadata-only API，不具备 active transform。
5. 示例配置不得包含真实密钥；凭据应保存到操作系统凭据库并只在配置中保留引用。

## 验证

```bash
cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
```

当前代码事实和发布缺口见 `docs/product/IMPLEMENTATION_STATUS.md`。
