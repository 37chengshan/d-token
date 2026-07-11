# d-token 当前实现状态

更新时间：2026-07-11 晚间
状态类型：代码事实快照，不替代 `docs/PRD.md` 与 `docs/PLAN.md`

## 1. 当前结论

d-token 已经不是 M1 占位工程。当前仓库包含可编译的 Rust-owned 本地 runtime、Tauri 2 桌面壳和 React 18/TypeScript 控制面，但仍处于发布阻断状态：本地路由、配置草稿、凭据引用、服务生命周期、Doctor、Dashboard、Event Ledger 和回退基础已经存在；第三方 Agent 的完整事务式自动配置、全平台候选版证据、长期持久化硬化和真实多 Agent 验收尚未闭环。

## 2. 代码所有权

| 范围 | 当前定位 |
|---|---|
| `code/apps/desktop/` | 唯一正式桌面 App；Tauri 2 + React 18 + TypeScript + Vite |
| `code/crates/` | 13 个 Rust 业务 crate，负责类型、配置、路由、压缩、进程、Doctor、事件、策略、API 与 proxy |
| `code/apps/desktop/src-tauri/` | 桌面 native command、凭据存储、配置草稿/应用、服务生命周期、安装事务、Doctor 与导出 |
| `code/runtimes/headroom/` | Headroom-derived 能力迁移与 provenance 边界，不是 Python sidecar 或外部服务依赖 |
| `code/UI design for project/` | 设计原型/历史 UI 输入，不是生产代码入口 |
| `code/apps/desktop/src-old/` | 旧前端实现，不得作为新功能落点 |
| `code/packages/sdk/` | 尚未实现的 SDK 预留目录，不是当前对外能力 |

## 3. 当前 Rust workspace

`code/Cargo.toml` 当前注册 13 个业务 crate 与 1 个 Tauri app crate：

```text
dtoken-types
dtoken-config
dtoken-router
dtoken-compress
dtoken-process
dtoken-doctor
dtoken-event-ledger
dtoken-preprocessor
dtoken-headroom-adapter
dtoken-project-policy
dtoken-config-patcher
dtoken-core-api
dtoken-proxy
d-token-desktop
```

当前生产主链路：

```text
AI coding agent
→ 127.0.0.1 d-token listener
→ dtoken-core-api / dtoken-proxy
→ compression + route selection
→ configured upstream
→ redacted runtime evidence / Event Ledger / Dashboard / Doctor / App
```

## 4. 已存在的本地 API

`dtoken-core-api` 当前提供以下主要能力：

- OpenAI Chat Completions、OpenAI Responses、Anthropic Messages 入口。
- `/health`、`/dashboard/payload`、`/stats`、`/stats-history`、`/metrics`。
- Provider、Route、Instance、active config、chain status 与近期 route decision。
- Doctor、recent logs、JSON/CSV export、Event Ledger。
- CCR、memory/learn、output shaping、项目策略与兼容性信息。
- Headroom-derived 与 integrated RTK 的 metadata-only 状态面。
- Optical shadow preview、status、capabilities、lifecycle 与 recent events。

这些端点的存在只证明实现面存在，不自动等于已达到稳定 Beta。真实 release claim 仍需同一候选 commit 的构建、运行、路由、持久化和平台证据。

## 5. 当前 Tauri 能力

生产版 invoke 当前包含：

- App 设置读写。
- Provider credential 的操作系统凭据库写入/删除，配置中只保存引用。
- Route/provider 草稿保存、应用和 active config 状态读取。
- d-token active config 的 rollback point 与恢复。
- 工具检测、安装计划预览、d-token 服务状态、启动和停止。
- 安装事务、Doctor、产品 readiness 聚合、Dashboard 读取和脱敏导出。
- Agent route 配置验证与项目压缩策略推荐。

OpenCode workload、模型测试、Headroom capability smoke 等开发命令只在 debug build 注册；release build 不注册这些测试入口。

当前限制：

1. `install_tool` 对缺失的 RTK/Headroom 不执行真实下载，只做检测并安全失败。
2. 路由草稿和 active config 只写 d-token 自有状态，不代表已自动修改 OpenCode、Claude Code、Codex、Cursor、Cline 或 cc-switch 配置。
3. 外部 Agent 配置恢复仍需完整事务、并发修改检测和故障注入证据。
4. 凭据库代码路径存在，但真实认证 Provider 的 macOS/Windows 候选版证据仍需补齐。

## 6. 当前桌面信息架构

正式一级导航与 PRD 一致：

1. 总体概览。
2. 安装与连接工作台。
3. 分析。
4. Doctor。
5. 变更与回退。
6. 设置。

Advanced、Logs/Security、Compatibility、Smart Learning 等仍作为工作台子页面、内部页面组件或现有实现来源存在，不应重新扩张为工程化顶级导航。

浏览器模式只能作为明确标识的 UI 预览；真实机器状态必须来自 Tauri command 与 d-token-owned local service。页面不得把 fixture、preview 或缺失数据渲染成真实运行状态。

## 7. Optical / Hybrid Context Optimization

当前已实现的是默认关闭、shadow-only、metadata-only 的规划与可观测基础：

- 版本化 Capability Snapshot 与 revoked/superseded 生命周期。
- Anthropic/OpenAI 工具调用依赖图与 fail-closed 规则。
- request-local TextArena、风险分类、geometry 与 economics 计划。
- raw-byte/canonical JSON fingerprint 计划证据。
- preview/status/capabilities/lifecycle/events API。

当前没有 PNG renderer、真实 request patcher、active mutation、post-execution validator、rollback executor 或 history collapse。任何文档和 UI 都不得宣称 Optical 已能在生产请求中主动省 token。

## 8. 仍然阻断发布的事项

1. 第三方 Agent 配置必须形成完整的 plan → diff → backup → confirm → apply → verify → rollback 事务。
2. 真实 OpenCode routed test 需要绑定当前候选 commit、实际配置端口、runtime identity、route decision 与脱敏 ledger 证据。
3. JSONL 仍需跨进程锁、轮转、背压、损坏隔离和查询持久化硬化。
4. Provider 并发、超时、流式桥接、错误分类、熔断和无静默 fallback 需要更完整矩阵。
5. macOS 需要真实窗口、安装后 runtime、Keychain、签名/notarization 与 artifact hash 的同 commit 证据。
6. Windows 需要 NSIS 安装、路径/UTF-8、凭据、回退、签名与 UI 一致性实机证据。
7. `code/UI design for project/` 与 `src-old/` 等旧实现需要在后续代码清理任务中决定保留、迁移或删除，本轮仅明确所有权。

## 9. 当前验证入口

```bash
python3 support/scripts/check_structure.py

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

2026-07-11 晚间本轮实际执行结果：

- `npm run check:structure`：通过，单一 PRD、活动目录、归档边界和本地 Markdown 链接有效。
- `npm run test:run`：14 个测试文件、63 个测试通过。
- `npm run build`：Vite 生产构建通过。
- `cargo test --workspace`：全 Rust workspace 通过；仍输出既有的非失败噪声 `kill: 999999: No such process`。
- `cargo fmt --all -- --check`：本轮尝试执行，但被本地 CodexPro safe bash allowlist 拒绝，未取得结果。
- `cargo clippy --workspace --all-targets -- -D warnings`、`npx tsc --noEmit`、`npm run test:e2e`：本轮未重跑，不应沿用旧报告作为当前证据。

Tauri 构建必须通过 `npm run tauri:dev` 或 `npm run tauri:build`，以便先准备对应平台的 `dtoken-proxy` external binary。
