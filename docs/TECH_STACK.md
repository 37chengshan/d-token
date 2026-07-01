# Desktop 技术选型决策

状态：Accepted  
关联：`docs/PRD.md`

## 1. 最终决策

采用：

```text
Tauri 2 + React + TypeScript + Vite
Rust core + Python sidecar transitional architecture
SQLite + local JSON/YAML config
```

Electron 仅作为备选，不作为主路线。

## 2. 为什么选 Tauri 2

Tauri 适合 d-token 的原因：

1. d-token 需要 Windows/macOS 原生 App，而不是纯网页。
2. d-token 需要管理本地 proxy、router、compress、doctor 等 sidecar。
3. d-token 需要轻量、低资源占用、本地优先。
4. d-token 的核心能力不在 UI，而在本地进程编排与可观测性。
5. Tauri 2 支持 external binaries / sidecar，适合把不同本地服务打包进 App。
6. Tauri updater 支持 Windows/macOS 更新包与签名信息，适合后续自动更新。

## 3. 为什么不是 Electron 主路线

Electron 的优点：

1. 前端生态成熟。
2. 开发速度快。
3. 很多成熟桌面应用验证过。

但 d-token 不首选 Electron：

1. Electron 基于 Chromium 多进程模型，整体更重。
2. Electron 主进程、渲染进程、preload、IPC 安全面更复杂。
3. d-token 的重点是本地代理和进程编排，不需要完整 Chromium 运行时。
4. 长期产品化需要更轻的桌面壳和更强的本地权限边界。

结论：Electron 可作为 fallback，但不是主线。

## 4. Rust 与 Python 的职责划分

### 4.1 Rust 负责

1. Desktop shell backend。
2. Local API server。
3. Process manager。
4. Port manager。
5. ProviderProfile / RouteProfile loader。
6. Request log writer。
7. Healthcheck / doctor。
8. Proxy/router skeleton。
9. Chain bridge。
10. Windows/macOS OS integration。

### 4.2 Python sidecar 负责

1. Headroom-compatible compression pipeline。
2. ContentRouter。
3. SmartCrusher。
4. CodeCompressor。
5. CCR。
6. Memory。
7. Learn。
8. Output shaper。
9. Evals / benchmark。
10. MCP compatibility。

### 4.3 长期迁移方向

稳定且平台相关的模块逐步 Rust 化：

1. Proxy。
2. Router。
3. Metrics ledger。
4. Config。
5. Logs。
6. Process manager。

复杂压缩和 ML 相关模块可继续保留 Python sidecar，直到 Rust parity 充分验证。

## 5. 前端技术

采用：

```text
React + TypeScript + Vite
```

理由：

1. 适合迁移 Headroom Dashboard 的复杂卡片、表格、历史视图和实时 feed。
2. TypeScript 更适合维护 ProviderProfile、RouteProfile、Instance、RequestLog 等复杂类型。
3. Vite 与 Tauri 集成成熟。
4. 后续可用 shadcn/ui、Tailwind 或自研组件库。

## 6. 数据存储

采用：

```text
SQLite + append-only JSONL ledger + local config files
```

用途：

1. SQLite：Dashboard 查询、历史统计、request index。
2. JSONL ledger：durable savings events、request events、debug events。
3. YAML/JSON config：provider、route、instance、developer config。
4. OS keychain：正式保存敏感凭据。

## 7. Windows/macOS 技术要求

### 7.1 Windows

1. WebView2 runtime 检测。
2. Microsoft C++ Build Tools / MSVC 检测。
3. `x86_64-pc-windows-msvc` toolchain。
4. `.exe` / `.cmd` / `.bat` shim 兼容。
5. UTF-8 日志。
6. 盘符路径兼容。
7. 端口占用检测。
8. 旧进程残留清理。
9. installer 签名预留。

### 7.2 macOS

1. Xcode Command Line Tools 检测。
2. Intel / Apple Silicon universal 或双包策略。
3. Keychain 预留。
4. LaunchAgent 预留。
5. quarantine / xattr 提示。
6. code signing / notarization 预留。
7. menu bar / tray 预留。

## 8. Sidecar 管理原则

1. 每个 sidecar 必须有 instance id。
2. 每个 sidecar 必须有 healthcheck。
3. 每个 sidecar 必须有 log path。
4. 每个 sidecar 必须可启动、停止、重启。
5. 每个 sidecar 必须可在 Dashboard 中观察。
6. 不允许用户看不到的后台进程长期残留。

## 9. 技术风险

| 风险 | 判断 | 缓解 |
|---|---|---|
| Headroom 功能太多 | 高 | Python sidecar 先保留 parity |
| Rust 全量重写周期过长 | 高 | 不一开始全 Rust 重写 |
| Dashboard 全量迁移工作量大 | 高 | M2 先做全卡片 mock，占位不删项 |
| Windows 端口/路径/编码问题 | 高 | Doctor 页面优先做 |
| macOS 签名与权限 | 中 | M5 再做签名/notarization |
| sidecar 打包复杂 | 中 | 严格按 target triple 管理 |

## 10. 决策结论

主路线固定为：

```text
Tauri 2 Desktop
+ React/TypeScript Dashboard
+ Rust orchestration core
+ Python Headroom-compatible sidecar
+ Local-first security boundary
```

后续除非出现重大技术阻断，不再切换到 Electron 主路线。
