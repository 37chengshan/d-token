# AGENTS.md

本文件给 Codex、OpenAI Agent、Claude Code、Cline 等 AI coding agents 使用。

## 项目定位

d-token 是本地优先的 AI Coding 压缩路由桌面 App，核心能力包括：

- Claude/Codex 官方上游支持。
- OpenAI-compatible / Anthropic-compatible 第三方上游支持。
- 先压缩再转发到 provider 或 cc-switch。
- Windows/macOS 可视化 App。
- 多实例、多端口、日志与 token savings 可观测。

## 工作原则

1. 不直接复制 headroom、cockpit-tools 或其他开源项目代码。
2. 只借鉴产品、架构、UX、目录组织与工程经验。
3. 所有密钥配置必须走本地 env、OS keychain 或 local config，不得提交真实 key。
4. 所有本地服务默认绑定 `127.0.0.1`。
5. 所有 JSON/JSONL/YAML/Markdown 读写默认 UTF-8。
6. Windows/macOS 是第一优先级。
7. 新增目录前先更新 `scripts/check_structure.py` 或说明理由。

## 目录边界

- `apps/desktop/`：桌面 App。
- `crates/dtoken-proxy/`：本地代理。
- `crates/dtoken-router/`：路由决策。
- `crates/dtoken-compress/`：压缩模块。
- `sidecars/cc-switch-bridge/`：cc-switch 联动。
- `packages/sdk/`：共享类型与 SDK。
- `configs/`：配置样例，不放真实密钥。
- `docs/`：PRD、架构、ADR、调研。
- `scripts/`：开发与 CI 辅助脚本。

## 修改前必读

- `docs/prd/d-token-prd-v0.1.md`
- `docs/architecture/system-architecture.md`
- `docs/decisions/ADR-0001-product-scope.md`
- `configs/providers.example.yaml`

## 禁止事项

- 不提交真实 API key、token、cookie、auth.json。
- 不把第三方 provider 静默回落到官方 provider。
- 不让请求链路不可观测。
- 不引入需要云端账号托管的设计。
- 不把 CLI 作为唯一入口；桌面 App 是主入口。
