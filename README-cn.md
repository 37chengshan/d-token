<div align="center">
  <img src="icons/logo.svg" width="112" alt="d-token 图标" />

# d-token

**让 AI Coding 更省 token，让每一次请求都看得见、可理解、可控制。**

一个本地桌面应用：统一连接 AI 编程 Agent、查看 token 消耗、预览优化策略，并在一个界面里安全管理配置变更。

[English](README.md) · 简体中文

[![CI](https://github.com/37chengshan/d-token/actions/workflows/ci.yml/badge.svg)](https://github.com/37chengshan/d-token/actions/workflows/ci.yml)
![Version](https://img.shields.io/badge/version-0.2.0--beta.1-ea580c)
![Status](https://img.shields.io/badge/status-beta-ea580c)
![Desktop](https://img.shields.io/badge/desktop-Tauri%202-24c8db?logo=tauri&logoColor=white)
![Core](https://img.shields.io/badge/core-Rust-b7410e?logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-Apache--2.0-2563eb)
</div>

---

> **公开未签名预发布，不是正式签名安装包发布。** `0.2.0-beta.1` 只有在同一 tag 的聚合发布 workflow 中同时完成 **macOS ARM64** 与 **Windows x64** 两个平台构建，并产出匹配的 SHA-256、SBOM、license、安全扫描与 provenance 证据后，才会作为 GitHub 公开预发布资产出现。该预发布仍然是未签名的：macOS 资产没有 Developer ID 签名或 notarization，Windows 资产没有 Authenticode 签名。未签名或 ad-hoc 签名的 macOS 构建可能出现系统安全警告；这属于预期行为。请核对仓库来源与已提供的哈希，不要为来自未知来源的安装包绕过系统安全保护。

## 概述

AI 编程工具会不断消耗上下文，但用户往往很难看清：token 到底花在了哪里、请求走了哪条路径、一次优化是否真的有效，以及配置出错后应该如何恢复。

d-token 把这些日常操作集中到一个桌面应用中。你可以连接受支持的 Agent，选择自己的模型服务，查看 token 减少与请求错误，在应用策略前先预览压缩效果，并在配置出现问题时恢复到之前的状态。

目标很简单：**在不打乱原有 AI Coding 工作流的前提下，使用更少的 token，并减少手动修改配置文件带来的麻烦。**

## 核心能力

| 能力 | 能为你带来什么 |
|---|---|
| **看得见的 token 节省** | 在一个页面查看请求量、估算或实测的 token 减少、压缩比例、趋势与错误。 |
| **统一连接 AI 编程 Agent** | 从一致的桌面流程检测并连接受支持的 Agent，不再分别手动修改多份配置。 |
| **清晰的请求路径** | 看清请求经过哪条 Route、连接到哪个上游，并主动调整路径，避免不透明的自动切换。 |
| **先预览，再决定是否调整** | 通过 Shadow 预览查看压缩策略和预期影响，再决定是否应用受支持的配置变化。 |
| **用更少上下文理解代码库** | 建立项目索引与代码图谱，让 Agent 更聚焦相关文件和依赖关系，减少反复传入无关上下文。 |
| **更安心的本地配置变更** | 在应用前查看差异、创建备份、验证结果，并对受支持的配置执行回退。 |
| **本地问题诊断** | 检查服务状态、端口、Agent 连接、配置健康度以及需要处理的问题。 |
| **适合长期使用的界面** | 支持浅色、深色和跟随系统主题，并可分别设置界面强调色与分析图表主色。 |

## 产品预览

### 1. 面向 AI Coding 的本地优先控制面

在一个本地桌面控制面查看 runtime 状态、受支持 Agent 的连接、Route 证据、token 测量与诊断。可用性以证据为准：不可用或部分完成的页面会如实标注，不会用示例成功数据填充。

<p align="center">
  <img src="assets/marketing/01-local-first-control-plane.svg" width="100%" alt="d-token 本地优先控制面" />
</p>

### 2. 显式路由，不静默切换服务商

d-token 会为受支持链路记录已选择的 Route 与上游。Route 不可用时不会静默回退到其它服务商；未完成的集成会保持 blocked，或要求用户明确操作。

<p align="center">
  <img src="assets/marketing/02-explicit-routing-pipeline.svg" width="100%" alt="d-token 显式路由链路" />
</p>

### 3. 可观测性与代码智能，但边界明确

查看脱敏 Event Ledger 证据、分析、诊断与受支持的代码智能状态。代码图谱属于实验性能力，需要用户明确导入并校验组件；视觉压缩坚持先预览，生产 Active 变换保持关闭。

<p align="center">
  <img src="assets/marketing/03-observability-and-code-intelligence.svg" width="100%" alt="d-token 可观测性与代码智能" />
</p>

> 页面中的估算节省用于帮助判断策略，不等于模型服务商账单。metadata-only、仅预览、disabled、blocked 或 unavailable 的能力，都不等价于生产可用功能。

### 4. 安全配置：审阅、备份、验证与回退

对受支持的配置操作，d-token 会展示用户可读摘要与脱敏 Diff，创建备份，要求确认，验证结果并保留回退证据。对不受支持的 Agent、修复、路由或变换操作，应用会保持 unavailable，而不是猜测写入配置。

<p align="center">
  <img src="assets/marketing/04-safe-config-and-rollback.svg" width="100%" alt="d-token 安全配置与回退" />
</p>

> OpenCode 当前拥有最强的已验证配置与回退路径。其它 Agent 与平台路径可能是 partial、disabled 或 unavailable。

## 如何融入你的工作流

```text
你正在使用的 AI 编程 Agent
              ↓
            d-token
     连接 · 优化 · 路由 · 观察
              ↓
      你配置的模型服务商
```

你仍然使用自己熟悉的 AI 编程工具。d-token 在本机连接 Agent 与你选择的模型服务，并在桌面应用中展示请求记录、token 数据、连接路径、诊断结果与受支持的恢复操作。

## 快速开始

### 当前可用状态

`0.2.0-beta.1` 只有在匹配 tag 的 GitHub 公开预发布中出现时，才算**公开未签名预发布**。它仍不是已签名、已公证的正式安装包发布。请不要把聊天附件、网盘、未知镜像或任意未签名本机构建当作官方预发布资产。

预发布目标覆盖 macOS ARM64 与 Windows x64。macOS 可能因未签名或 ad-hoc 签名出现安全警告。Windows 资产同样保持未签名，不宣称 Authenticode 或 SmartScreen reputation。正式签名、公证与更广泛的平台验收仍是发布阻断项。

### 环境要求

- Node.js 24 与 npm
- Rust stable 工具链
- macOS ARM64 或 Windows x64 桌面开发环境
- macOS 需要 Xcode Command Line Tools
- Windows 需要 Tauri 与 WebView2 对应环境

### 启动桌面应用

```bash
git clone https://github.com/37chengshan/d-token.git
cd d-token/code/apps/desktop
npm ci
npm run tauri:dev
```

`npm run tauri:dev` 会先准备本地运行组件，再启动桌面应用。单独运行前端只能预览界面，不能连接本机 Agent、服务或配置。

### 构建本地候选

```bash
cd code/apps/desktop
npm run tauri:build
```

本地构建成功不代表安装包已经完成正式签名或具备公开分发条件。

## Beta 使用说明

- 这是公开未签名预发布，不代表已完成 Developer ID 签名、公证、Gatekeeper 验证或 Authenticode 签名。
- OpenCode 当前拥有最完整的真实连接、请求路由与配置回退验证。Claude Code 与 Codex 已具备接入流程，但仍在进行同一候选版本的真实用户验收。
- 视觉压缩以 Shadow 预览为主，生产 Active 变换关闭；代码图谱为实验性能力，不会直接打包大型上游组件。
- 一部分 Headroom-derived 与 Agent 相关状态仍是 metadata-only、仅预览、静态兼容声明、partial、blocked 或 unavailable，不能据此宣称已完成完整 Headroom 能力迁移。
- token 减少可能来自实测数据，也可能来自当前证据下的估算；估算不能当作模型服务商账单。
- 受支持的配置写入遵循“预览 → 脱敏 Diff → 备份 → 确认 → 应用 → 验证 → 回退”。没有该事务能力的路径不应自动写入配置。

最新实现事实与发布阻断项以 [`docs/product/IMPLEMENTATION_STATUS.md`](docs/product/IMPLEMENTATION_STATUS.md) 为准。

## 隐私与本地数据

d-token 会把自身的设置、诊断信息、请求元数据和回退记录保存在本机。普通日志不会主动保存完整 prompt、完整 response、源代码正文、API key 或 Authorization 信息。

你选择的模型服务商仍会收到你主动发送给它的请求。d-token 不会把第三方模型服务自动变成本地服务，也不能替代你对服务商隐私条款的判断。

## 技术栈

Tauri 2 · React 18 · TypeScript · Vite · Rust · SQLite

## 开发与文档

- [产品契约](docs/PRD.md)
- [当前实现状态](docs/product/IMPLEMENTATION_STATUS.md)
- [总体架构](docs/ARCHITECTURE.md)
- [安全边界](docs/SECURITY_BOUNDARIES.md)
- [版本记录](CHANGELOG.md)
- [安全策略](SECURITY.md)
- [第三方声明](THIRD_PARTY_NOTICES.md)

欢迎通过 GitHub Issues 和 Pull Requests 提交问题、建议与改进。进行仓库级修改前，请先阅读 [`AGENTS.md`](AGENTS.md)。

## 许可证

项目采用 [Apache License 2.0](LICENSE)。打包或适配的第三方组件说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

---

<div align="center">
  <strong>d-token</strong><br />
  省下 token，看清变化，保持掌控。
</div>
