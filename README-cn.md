<p align="center">
  <img src="assets/marketing/hero-banner.png" width="100%" alt="d-token — 更少上下文。同样意图。完全掌控。" />
</p>

<div align="center">
  <img src="assets/marketing/app-icon.png" width="112" alt="d-token 图标" />

# d-token

面向 AI 编码 Agent 的本地上下文控制面。

[English](README.md) · 简体中文

[![CI](https://github.com/37chengshan/d-token/actions/workflows/ci.yml/badge.svg)](https://github.com/37chengshan/d-token/actions/workflows/ci.yml)
![Version](https://img.shields.io/badge/version-0.2.0--beta.1-ea580c)
![Status](https://img.shields.io/badge/status-beta-ea580c)
![Desktop](https://img.shields.io/badge/desktop-Tauri%202-24c8db?logo=tauri&logoColor=white)
![Core](https://img.shields.io/badge/core-Rust-b7410e?logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-Apache--2.0-2563eb)
</div>

---

## 亮点

- 🧠 **看得见的节省** — 单次真实路由请求物理减少 **33,705 token**（实测，2026-08-04）
- 🛣️ **显式路由** — 每个请求都走明确路径到达你配置的上游；绝不静默切换服务商
- 🧾 **每次请求一张回执** — 来源 Agent、路由、变换、Local 与 Provider Token、恢复状态
- 🔒 **安全配置** — 预览 → 脱敏 Diff → 备份 → 验证 → 回退
- 🏠 **本地优先** — 在本机运行；秘密信息绝不进入普通日志
- 🩺 **配置的医生** — 出问题时可以诊断、修复、恢复

## 它是什么

d-token 运行在你的 AI 编码 Agent 与你选择的模型服务之间。它能看到每一个请求，在上下文离开本机前压缩冗余内容，保持路由显式，并让配置变更可恢复——而不是一堆脚本和猜测。

```
你正在使用的 AI 编程 Agent
              ↓
            d-token
     连接 · 优化 · 路由 · 观察
              ↓
      你配置的模型服务商
```

## 快速开始

```bash
git clone https://github.com/37chengshan/d-token.git
cd d-token/code/apps/desktop
npm ci
npm run tauri:dev
```

> [!NOTE]
> `0.2.0-beta.1` 是面向 macOS ARM64 与 Windows x64 的**公开未签名预发布**——不是已签名安装包。未签名的 macOS 构建可能出现系统安全警告；请核对仓库来源与已提供的哈希，不要绕过系统安全保护。详见 [SECURITY.md](SECURITY.md)。

## 为什么需要它

AI 编码 Agent 会一遍又一遍重发相同的文件、日志和工具输出。token 账单在悄悄增长，但没人能说清楚：这些 token 花在了哪里、哪个 Provider 处理了请求、所谓的"优化"是否真的有效。d-token 让这三个问题都有答案。

## 路线图

| 版本 | 重点 | 状态 |
|---|---|---|
| `0.2.0-beta.1` | 可信连接与可信压缩、Optimization Receipt、Doctor | 当前候选 |
| `v0.3` | 上下文连续性：跨 Agent Context Relay、Session Cache、cc-switch 跟随 / 锁定 / 观察 | 规划中 |
| `v0.4` | 自动化本地控制面：多 Agent 适配、低风险自动修复、稳定签名 | 规划中 |

## 隐私

d-token 会把设置、诊断信息、请求元数据和回退记录保存在本机。普通日志不会主动保存完整 prompt、完整 response、源代码正文、API key 或 Authorization 信息。你选择的模型服务商仍会收到你主动发送给它的请求。

## 文档

- [产品契约](docs/PRD.md)
- [产品方向（v2.1）](docs/d-token_AI_product_target_spec_v2.md)
- [实现状态与发布门禁](docs/product/IMPLEMENTATION_STATUS.md)
- [总体架构](docs/ARCHITECTURE.md)
- [安全边界](docs/SECURITY_BOUNDARIES.md)
- [版本记录](CHANGELOG.md)

## 参与贡献

欢迎通过 GitHub Issues 和 Pull Requests 提交问题、建议与改进。进行仓库级修改前，请先阅读 [`AGENTS.md`](AGENTS.md)。

## 许可证

[Apache License 2.0](LICENSE)。打包或适配的第三方组件说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
