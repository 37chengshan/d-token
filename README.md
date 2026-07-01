# d-token

> 面向 Claude Code、Codex、OpenAI-compatible/Anthropic-compatible 上游的本地压缩、路由、可视化与多实例管理层。

## 项目定位

d-token 不是简单的 API 中转，也不是单一命令行压缩工具。它的目标是把以下能力合并成一个 Windows/macOS 可视化桌面应用：

1. **先压缩，再转发**：在请求进入官方上游、第三方上游或 cc-switch 之前，先完成上下文压缩、缓存对齐、可逆检索与日志记录。
2. **多上游统一路由**：同时支持 Claude/Anthropic、Codex/OpenAI 官方上游，以及 OpenAI-compatible / Anthropic-compatible 第三方上游。
3. **cc-switch 链式转发**：当用户需要通过 cc-switch 做二次路由时，d-token 作为前置压缩层，链路为 `Agent -> d-token -> cc-switch -> Provider`。
4. **桌面化取代 Web Dashboard 与命令行安装**：以 Windows/macOS App 形式提供一键安装、端口管理、路由管理、日志、用量、压缩率、实例启停与中文界面。
5. **多账号/多实例联动**：借鉴 cockpit-tools 的多账号、多实例、配额监控与 sidecar 设计，支持官方登录实例和第三方/中转实例并行运行。

## v0 仓库结构

```text
.
├── README.md
├── docs/
│   ├── prd/
│   │   └── d-token-prd-v0.1.md
│   ├── architecture/
│   │   └── system-architecture.md
│   ├── decisions/
│   │   └── ADR-0001-product-scope.md
│   └── research/
│       └── open-source-reference.md
├── apps/
│   └── desktop/
│       └── README.md
├── crates/
│   ├── dtoken-proxy/
│   ├── dtoken-router/
│   └── dtoken-compress/
├── packages/
│   └── sdk/
├── sidecars/
│   └── cc-switch-bridge/
├── configs/
│   └── providers.example.yaml
└── scripts/
    ├── check_structure.py
    └── dev/
        └── README.md
```

## 当前阶段

当前仓库处于 **0 -> 1 产品定义与骨架初始化阶段**。优先级如下：

- P0：PRD、架构、配置模型、目录边界、开发约束。
- P1：本地代理内核、Provider Adapter、cc-switch bridge、多端口实例管理。
- P2：Windows/macOS 桌面 App、中文可视化 Dashboard、日志与用量面板。
- P3：与 cockpit-tools/外部账号管理工具联动。

## 重要原则

- 不直接复制其他开源项目代码；只做产品、架构、交互与能力借鉴。
- 所有密钥只保存在本地，不上传云端。
- 默认监听 `127.0.0.1`，不暴露公网服务。
- 明确区分官方上游、第三方上游、cc-switch 二次路由与本地压缩层。
- Windows/macOS 是第一优先级；Linux 可作为后续兼容目标。
