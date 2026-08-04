# Desktop 技术选型决策

状态：Accepted
最后校准：2026-07-11
关联：`docs/PRD.md`

## 1. 接受的主路线

```text
Rust 核心业务与本地服务
+ Tauri 2 桌面控制面
+ React 18 / TypeScript / Vite UI
+ 极少量 Swift / macOS 系统桥接（仅在 Rust/Tauri 无可靠能力时）
+ 本地 JSON/JSONL 与 OS 凭据存储
```

Windows 与 macOS 使用同一套 React UI、信息架构、数据契约和用户任务语义。平台差异只能存在于凭据、进程、路径、签名、安装器等系统适配层。当前先在 macOS 完成真实窗口和功能验收，但 Windows 始终是一等支持平台。

Electron 仅是重大技术阻断时的备选，不是并行主路线。Python 可以用于 `support/` 下的开发或审计脚本，但不作为成品运行时、压缩引擎或桌面依赖。

## 2. 为什么选择 Tauri 2

1. d-token 需要 Windows/macOS 原生桌面控制面和受控本地权限。
2. 核心工作发生在本地代理、压缩、路由、配置事务、Doctor 和 Event Ledger，而非浏览器运行时。
3. Tauri 2 能以轻量 WebView 承载共享 React UI，并通过 Rust command 暴露窄而可审计的系统能力。
4. `dtoken-proxy` 可以作为按 target triple 打包的 Rust external binary，仍属于 d-token-owned Rust runtime。
5. Tauri bundle、签名和 updater 能覆盖 Windows/macOS 的发布工程需求。

## 3. 代码职责

### 3.1 Rust 核心

正式代码位于 `code/crates/*` 与 `code/apps/desktop/src-tauri`，负责：

1. OpenAI/Anthropic 本地协议入口与流式转发。
2. Headroom-derived compression、CCR、memory、output shaping，以及 Rust-native Protected Context IR、semantic/Optical hybrid engine、PNG renderer 和 validator。
3. Provider/Route/Instance 决策、凭据短生命周期解引用和 upstream 转发。
4. Event Ledger、metrics、stats、history、export 和 Doctor。
5. 配置计划、备份、应用、验证、回退与进程生命周期。
6. Tauri commands、平台路径、OS credential store 和桌面打包。

`code/runtimes/headroom/` 是 Headroom-derived 能力迁移的 provenance/兼容边界，不是外部服务依赖，也不是 Python sidecar 入口。仓库中不存在 `code/sidecars/*`，架构不得引用这些路径。

### 3.2 React/TypeScript UI

当前依赖事实是 React 18.3、TypeScript 5.7、Vite 6、Vitest 3。UI 负责：

1. 总览、安装与连接、Agents、上下文优化、分析、Doctor、日志、变更与回退、设置。
2. 统一 runtime evidence 状态，清楚区分真实、不可用、过期和浏览器预览。
3. 把 native/API 能力翻译为用户任务；正式 UI 不暴露 fixture、benchmark、smoke 等开发术语。
4. Windows/macOS 1:1 的视觉系统、布局、文案与交互语义。

浏览器模式只用于 UI 预览，不能证明真实能力；桌面窗口必须由 Tauri command 与 d-token-owned local service 提供运行事实。

### 3.3 Swift/macOS 桥接

默认优先 Rust crate、Tauri plugin 或系统命令。仅当这些路径无法可靠实现且有明确测试边界时，才允许引入少量 Swift，例如特定 AppKit 生命周期或系统 UI 桥接。Swift 不承载业务规则、路由、压缩、配置事务或数据模型。

## 4. 数据与凭据

当前实现：

- JSON/JSONL：配置、RequestLog、RouteDecision、Event Ledger、CCR metadata 与回退证据。
- macOS Keychain / Windows Credential Manager：由 Rust `keyring` 适配写入和读取 Provider 凭据；配置与 API 只保存/返回不透明引用或 `credential_configured`。
- 完整 prompt/response、密钥、Authorization、cookie 不进入日志、Ledger 或导出。

目标但尚未完成：SQLite 查询索引、JSONL 坏行隔离/轮转/锁/损坏诊断。文档和 UI 在这些门禁完成前不得宣称长期持久化已达到发布级。

## 5. 平台要求

### Windows

1. WebView2、MSVC toolchain、UTF-8、盘符/UNC 路径与 `.exe/.cmd/.bat` shim。
2. Windows Credential Manager、端口/进程身份检查、NSIS/MSI 与代码签名。
3. 共享 React UI；Windows 专属逻辑只位于 Rust/Tauri 平台适配和构建配置。

### macOS

1. Xcode Command Line Tools，Apple Silicon/Intel/universal 构建策略。
2. Keychain、quarantine/xattr、受管进程、`.app`/DMG、签名与 notarization。
3. 当前 macOS 平台覆盖文件启用 `.app` 与 DMG；公开发布 workflow 强制签名、notarization、staple、Gatekeeper 与 SHA-256 manifest，但完成发布仍需真实 Apple 凭据和同一候选 commit 的执行证据。

## 6. Rust external binary 管理

`dtoken-proxy`、metadata-only `dtoken-agent-bridge` 与受限 `dtoken-codebase-memory-launcher` 是 d-token 自有 Rust external binaries，不是外部 Headroom/RTK 服务。Codebase Memory 大型 headless upstream 不进入 App bundle；用户从页面固定的官方 `v0.9.0` release 自行下载并解压到默认或自定义目录，App 自动发现后由 launcher 在批准项目、私有 cache 和零仓库写入策略下启动：

1. 按 target triple 生成并由 Tauri `externalBin` 打包。
2. 必须有受管 identity、固定 loopback bind、healthcheck、日志和生命周期证据。
3. 启停失败必须显式，禁止留下无法安全识别的孤儿进程。
4. 开发期不得启动、停止或占用外部 Headroom/RTK/cc-switch 服务。
5. d-token 不下载、安装或更新 Codebase Memory upstream；运行时只接受 d-token manifest 固定的平台/架构/hash，目录变化通过 metadata fingerprint 触发重新验证。版本升级必须先随 d-token 更新 manifest/allowlist，再由用户重新导入；原生 upstream UI 不进入 bundle。

## 7. 当前风险与发布门禁

| 风险 | 当前事实 | 门禁 |
|---|---|---|
| 安装/配置事务 | 有计划、draft、backup/rollback 基础，尚未覆盖全部原子写入和故障恢复 | 权限、磁盘满、并发修改、验证失败等故障注入全部可恢复 |
| Runtime truth | Dashboard/Tauri 已能读取 d-token local service，但部分桌面聚合数据仍需去硬编码 | 单一 evidence contract，UI 不再把偏好或预览显示成运行事实 |
| 持久化 | JSONL 可用，SQLite/轮转/坏行隔离未闭环 | 长期运行、损坏恢复、并发和导出门禁通过 |
| macOS 发布 | fail-closed bundle/sign/notarize/Gatekeeper/hash workflow 已配置 | 真实 Apple 凭据执行并绑定同一 commit 的安装后运行和 OpenCode 证据 |
| Windows 发布 | fail-closed Authenticode/NSIS 安装启动/hash workflow 已配置，实机证据不足 | Windows runner 真实签名、凭据、路径、回退与视觉一致性验收 |
| Optical | active 后端链已实现但默认关闭，仅限同协议 Anthropic closed historical tool result | 真实模型 signed eval、同候选版 routed evidence 和全部资源/安全门通过前禁止默认 active |

## 8. 结论

唯一接受的产品架构是：

```text
Tauri 2 Desktop
+ shared React/TypeScript UI
+ d-token-owned Rust core/runtime
+ minimal platform adapters (Swift only when unavoidable on macOS)
+ local-first security boundary
```

不得再把 Python sidecar、外部 Headroom 8787 或不存在的 `code/sidecars/*` 写成当前产品运行架构。
