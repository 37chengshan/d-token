# d-token

> 面向 AI Coding 的本地优先桌面控制面与 Rust runtime。

更新时间：2026-07-11 晚间

d-token 在 Windows/macOS 上把本地 AI 请求入口、上下文优化、Provider/Route、运行状态、Doctor、Event Ledger、配置变更与回退统一到一个 Tauri 桌面应用中。项目目标是让用户明确知道流量经过哪里、配置将改什么、数据是否真实、出现问题如何诊断和恢复。

## 当前状态

仓库已经包含可编译的 Rust workspace、Tauri 2 桌面壳和 React 18/TypeScript UI，不再是占位项目。当前已有：

- OpenAI Chat Completions、OpenAI Responses 和 Anthropic Messages 本地入口。
- 显式 Provider/Route/Instance 配置和无静默 fallback 的路由基础。
- d-token-owned Dashboard、stats/history/metrics、Doctor、logs、Event Ledger 和脱敏导出。
- d-token route 草稿、active config、rollback point 和本地配置恢复。
- 操作系统凭据库接入，配置只保存不透明 credential reference。
- App-managed `dtoken-proxy` Rust runtime 的状态、启动、停止和身份验证。
- macOS/Windows Tauri sidecar 准备与平台 bundle 配置。
- Optical / Hybrid Context Optimization 的 shadow planner、Preview API 和 Capability 生命周期可观测面。

当前仍不是可无条件公开发布的稳定版本。以下能力尚未闭环：

- 第三方 Agent 配置的完整 plan → diff → backup → apply → verify → rollback 事务。
- 缺失 RTK/Headroom 时的可信下载、签名校验和真实自动安装。
- 多 Agent、真实认证 Provider、长期持久化和故障注入的完整矩阵。
- macOS 签名/notarization、Windows 签名和同一候选 commit 的实机发布证据。
- Optical PNG renderer、request patcher、active mutation、post-validator 和 rollback。

完整事实见 [`docs/product/IMPLEMENTATION_STATUS.md`](docs/product/IMPLEMENTATION_STATUS.md)。

## 产品页面

正式一级页面固定为：

1. 总体概览。
2. 安装与连接。
3. 分析。
4. Doctor。
5. 变更与回退。
6. 设置。

高级 Provider/Route、Smart Learning、日志安全和兼容性能力通过工作台子流程或内部页面承载，不作为工程化测试中心。

## 架构

```text
AI coding agent
→ 127.0.0.1 d-token listener
→ d-token-owned Rust runtime
→ compression / context optimization / route decision
→ configured upstream
→ redacted Event Ledger / metrics / Dashboard / Doctor / App
```

技术主线：

- Rust：核心业务、local API、proxy、路由、压缩、Doctor、事件和进程。
- Tauri 2：桌面 native command、平台适配、凭据和打包。
- React 18 + TypeScript + Vite：Windows/macOS 共用 UI。
- Python：仅用于 `support/` 中的开发与审计脚本，不是成品 runtime。

外部 Headroom、standalone RTK 和 cc-switch 不是 d-token App 的隐式运行依赖。`code/runtimes/headroom/` 只保存 Headroom-derived 能力的 provenance/迁移边界。

## 仓库结构

```text
AGENTS.md                       通用 AI coding agent 入口
CLAUDE.md                       Claude Code 入口
README.md                       项目概览
docs/                           当前规范、历史与归档
code/                           Rust workspace 与桌面 App
support/                        测试、脚本、fixture、CI 和审计账本
```

核心代码：

```text
code/apps/desktop/              唯一正式桌面 App
code/apps/desktop/src/app/      当前 React UI
code/apps/desktop/src-tauri/    Tauri commands 与平台打包
code/crates/                    13 个 Rust 业务 crate
```

`code/UI design for project/` 是设计原型，`code/apps/desktop/src-old/` 是旧实现，均不是生产功能落点。

## 开发

### 前端

```bash
cd code/apps/desktop
npm install
npm run dev
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
```

### Rust workspace

```bash
cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Tauri 桌面

```bash
cd code/apps/desktop
npm run tauri:dev
npm run tauri:build
```

`tauri:dev` 和 `tauri:build` 会先准备当前平台的 `dtoken-proxy` external binary。不要绕过准备脚本直接构建不完整 bundle。

### 文档与结构检查

```bash
python3 support/scripts/check_structure.py
```

## 文档入口

- [`docs/index.md`](docs/index.md) — 文档总导航。
- [`docs/PRD.md`](docs/PRD.md) — 唯一产品真理源。
- [`docs/PLAN.md`](docs/PLAN.md) — 当前执行计划。
- [`docs/product/IMPLEMENTATION_STATUS.md`](docs/product/IMPLEMENTATION_STATUS.md) — 当前代码事实。
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — 架构。
- [`docs/SECURITY_BOUNDARIES.md`](docs/SECURITY_BOUNDARIES.md) — 安全边界。
- [`docs/archive/README.md`](docs/archive/README.md) — 归档说明。

## 安全原则

- 默认只绑定 loopback。
- 禁止存储或提交真实密钥、token、cookie、Authorization、auth 文件和完整 prompt/response。
- 凭据只进入操作系统凭据库，不允许明文 fallback。
- 所有真实配置写入都必须有摘要、diff、备份、确认、验证和回退证据。
- 没有当前 runtime evidence 时，UI 必须显示 unavailable、partial 或 preview，不能冒充真实机器状态。

## 许可证

Apache-2.0。复用外部开源实现时必须遵守对应许可证、NOTICE、来源和修改记录要求。
